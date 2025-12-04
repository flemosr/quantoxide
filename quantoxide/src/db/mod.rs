use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;

use crate::db::{postgres::ohlc_candles::PgOhlcCandlesRepo, repositories::OhlcCandlesRepository};

pub(crate) mod error;
pub(crate) mod models;
mod postgres;
mod repositories;

use error::{DbError, Result};
use postgres::{price_ticks::PgPriceTicksRepo, running_trades::PgRunningTradesRepo};
use repositories::{PriceTicksRepository, RunningTradesRepository};

pub struct Database {
    pub(crate) price_ticks: Box<dyn PriceTicksRepository>,
    pub(crate) running_trades: Box<dyn RunningTradesRepository>,
    pub(crate) ohlc_candles: Box<dyn OhlcCandlesRepository>,
}

impl Database {
    pub async fn new(postgres_db_url: &str) -> Result<Arc<Self>> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(postgres_db_url)
            .await
            .map_err(DbError::Connection)?;

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(DbError::Migration)?;

        let pool = Arc::new(pool);
        let price_ticks = Box::new(PgPriceTicksRepo::new(pool.clone()));
        let running_trades = Box::new(PgRunningTradesRepo::new(pool.clone()));
        let ohlc_candles = Box::new(PgOhlcCandlesRepo::new(pool.clone()));

        Ok(Arc::new(Self {
            price_ticks,
            running_trades,
            ohlc_candles,
        }))
    }
}
