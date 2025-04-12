use std::sync::Arc;

use crate::{
    api::{
        websocket::models::{ConnectionState, LnmWebSocketChannel, WebSocketApiRes},
        ApiContext,
    },
    db::DbContext,
};

pub mod error;

use error::{RealTimeCollectionError, Result};

pub struct RealTimeCollectionTask {
    db: Arc<DbContext>,
    api: Arc<ApiContext>,
}

impl RealTimeCollectionTask {
    pub fn new(db: Arc<DbContext>, api: Arc<ApiContext>) -> Self {
        Self { db, api }
    }

    pub async fn run(self) -> Result<()> {
        let ws = self.api.connect_ws().await?;

        let mut receiver = ws.receiver().await?;

        let channels = vec![LnmWebSocketChannel::FuturesBtcUsdLastPrice];
        ws.subscribe(channels).await?;

        loop {
            match receiver.recv().await {
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
    }
}
