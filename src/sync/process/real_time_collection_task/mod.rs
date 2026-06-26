use std::{mem, sync::Arc};

use tokio::{
    sync::broadcast::{self, error::RecvError},
    task::JoinHandle,
};

use lnm_sdk::stream::v1::{
    StreamClient,
    models::{LastPrice, StreamTopic, StreamUpdate},
};

use crate::db::{Database, error::Result as DbResult, models::PriceTickRow};

pub(crate) mod error;

use error::{RealTimeCollectionError, Result};

pub(super) struct RealTimeCollectionTask {
    db: Arc<Database>,
    api_ws: Arc<StreamClient>,
    shutdown_tx: broadcast::Sender<()>,
    price_tick_tx: broadcast::Sender<PriceTickRow>,
}

impl RealTimeCollectionTask {
    pub fn new(
        db: Arc<Database>,
        api_ws: Arc<StreamClient>,
        shutdown_tx: broadcast::Sender<()>,
        price_tick_tx: broadcast::Sender<PriceTickRow>,
    ) -> Self {
        Self {
            db,
            api_ws,
            shutdown_tx,
            price_tick_tx,
        }
    }

    pub async fn run(self) -> Result<()> {
        let stream = self.api_ws.connect().await?;

        let mut stream_rx = stream.receiver().await?;

        stream
            .subscribe(vec![StreamTopic::FuturesInverseBtcUsdLastPrice])
            .await?;

        let mut shutdown_rx = self.shutdown_tx.subscribe();

        let mut pending_ticks: Vec<LastPrice> = Vec::new();
        let mut db_op: Option<JoinHandle<DbResult<Vec<PriceTickRow>>>> = None;

        loop {
            tokio::select! {
                biased;

                stream_res = stream_rx.recv() => {
                    match stream_res {
                        Ok(res) => match res {
                            StreamUpdate::FuturesInverseBtcUsdLastPrice(tick) => {
                                pending_ticks.push(tick);
                            }
                            StreamUpdate::ConnectionStatus(new_status) => {
                                if !new_status.is_connected() {
                                    return Err(RealTimeCollectionError::BadConnectionUpdate(new_status));
                                }
                            },
                            _ => {}
                        },
                        Err(RecvError::Lagged(skipped)) => {
                            return Err(RealTimeCollectionError::StreamRecvLagged { skipped });
                        },
                        Err(RecvError::Closed) => {
                            return Err(RealTimeCollectionError::StreamRecvClosed);
                        }
                    }
                }

                shutdown_res = shutdown_rx.recv() => {
                    if let Err(e) = shutdown_res {
                        return Err(RealTimeCollectionError::ShutdownSignalRecv(e));
                    }

                    // Wait for in-flight DB operation to complete
                    if let Some(handle) = db_op.take() {
                        let inserted_ticks = handle.await.expect("`add_ticks` must not panic")?;
                        for tick in inserted_ticks {
                            let _ = self.price_tick_tx.send(tick);
                        }
                    }

                    // Flush pending ticks before shutdown
                    if !pending_ticks.is_empty() {
                        let inserted_ticks = self.db.price_ticks.add_ticks(&pending_ticks).await?;
                        for tick in inserted_ticks {
                            let _ = self.price_tick_tx.send(tick);
                        }
                    }

                    return Ok(());
                }

                db_result = async {
                    db_op.as_mut().expect("`db_op` is `Some`").await
                }, if db_op.is_some() => {
                    db_op = None;
                    let inserted_ticks = db_result.expect("`add_ticks` must not panic")?;
                    for tick in inserted_ticks {
                        let _ = self.price_tick_tx.send(tick);
                    }
                }
            }

            // Start new DB operation if previous completed and we have pending ticks
            if db_op.is_none() && !pending_ticks.is_empty() {
                let ticks = mem::take(&mut pending_ticks);
                let db = self.db.clone();
                db_op = Some(tokio::spawn(async move {
                    db.price_ticks.add_ticks(&ticks).await
                }));
            }
        }
    }
}
