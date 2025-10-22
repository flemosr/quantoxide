use std::sync::Arc;

use tokio::sync::broadcast::{self, error::RecvError};

use lnm_sdk::api::{
    ApiContext,
    websocket::models::{LnmWebSocketChannel, WebSocketUpdate},
};

use crate::db::{DbContext, models::PriceTick};

mod error;

use error::Result;

pub use error::RealTimeCollectionError;

pub struct RealTimeCollectionTask {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
    shutdown_tx: broadcast::Sender<()>,
    price_tick_tx: broadcast::Sender<PriceTick>,
}

impl RealTimeCollectionTask {
    pub fn new(
        db: Arc<DbContext>,
        api: Arc<ApiContext>,
        shutdown_tx: broadcast::Sender<()>,
        price_tick_tx: broadcast::Sender<PriceTick>,
    ) -> Self {
        Self {
            db,
            api,
            shutdown_tx,
            price_tick_tx,
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
                            WebSocketUpdate::PriceTick(tick) => {
                                if let Some(new_tick) = self.db.price_ticks.add_tick(&tick).await? {
                                    let _ = self.price_tick_tx.send(new_tick);
                                }
                            }
                            WebSocketUpdate::PriceIndex(_index) => {}
                            WebSocketUpdate::ConnectionStatus(new_status) => {
                                if !new_status.is_connected() {
                                    return Err(RealTimeCollectionError::BadConnectionUpdate(new_status));
                                }
                            },
                        },
                        Err(RecvError::Lagged(skipped)) => return Err(RealTimeCollectionError::WebSocketRecvLagged{skipped}),
                        Err(RecvError::Closed) => return Err(RealTimeCollectionError::WebSocketRecvClosed)
                    }
                }
                shutdown_res = shutdown_rx.recv() => {
                    if let Err(e) = shutdown_res {
                        return Err(RealTimeCollectionError::ShutdownSignalRecv(e));
                    }
                    return Ok(());
                }
            }
        }
    }
}
