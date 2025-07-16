use std::sync::Arc;

use tokio::sync::broadcast;

use lnm_sdk::api::{
    ApiContext,
    websocket::{
        models::{LnmWebSocketChannel, WebSocketApiRes},
        state::ConnectionState,
    },
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
                            WebSocketApiRes::PriceTick(tick) => {
                                if let Some(new_tick) = self.db.price_ticks.add_tick(&tick).await? {
                                    let _ = self.price_tick_tx.send(new_tick);
                                }
                            }
                            WebSocketApiRes::PriceIndex(_index) => {}
                            WebSocketApiRes::ConnectionUpdate(new_state) => {
                                if !matches!(new_state.as_ref(), ConnectionState::Connected) {
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
