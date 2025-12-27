use std::sync::Arc;

use sqlx::postgres::PgPoolOptions;

pub(crate) mod error;
pub(crate) mod models;
mod postgres;
mod repositories;

use error::{DbError, Result};
use postgres::{
    ohlc_candles::PgOhlcCandlesRepo, price_ticks::PgPriceTicksRepo,
    running_trades::PgRunningTradesRepo,
};
use repositories::{OhlcCandlesRepository, PriceTicksRepository, RunningTradesRepository};

/// Primary database interface for market data persistence and retrieval.
///
/// Provides access to repositories for OHLC candle data, price tick data, and running trade
/// information. Uses PostgreSQL as the underlying storage engine with automatic migrations.
pub struct Database {
    pub(crate) ohlc_candles: Box<dyn OhlcCandlesRepository>,
    pub(crate) price_ticks: Box<dyn PriceTicksRepository>,
    pub(crate) running_trades: Box<dyn RunningTradesRepository>,
}

impl Database {
    /// Creates a new database instance and runs migrations.
    ///
    /// Establishes a connection pool to the PostgreSQL database and automatically applies any
    /// pending migrations. Returns an error if the connection fails or migrations cannot be
    /// applied.
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
        let ohlc_candles = Box::new(PgOhlcCandlesRepo::new(pool.clone()));
        let price_ticks = Box::new(PgPriceTicksRepo::new(pool.clone()));
        let running_trades = Box::new(PgRunningTradesRepo::new(pool.clone()));

        Ok(Arc::new(Self {
            ohlc_candles,
            price_ticks,
            running_trades,
        }))
    }
}
