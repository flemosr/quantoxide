use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use std::sync::Arc;

use lnm_sdk::api::websocket::models::PriceTickLNM;

use crate::db::error::DbError;

use super::super::{error::Result, repositories::PriceTicksRepository};

pub struct PgPriceTicksRepo {
    pool: Arc<Pool<Postgres>>,
}

impl PgPriceTicksRepo {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &Pool<Postgres> {
        self.pool.as_ref()
    }
}

#[async_trait]
impl PriceTicksRepository for PgPriceTicksRepo {
    async fn add_tick(&self, tick: &PriceTickLNM) -> Result<()> {
        sqlx::query!(
            r#"
                INSERT INTO price_ticks (time, last_price)
                VALUES ($1, $2)
                ON CONFLICT DO NOTHING
            "#,
            tick.time(),
            tick.last_price(),
        )
        .execute(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }
}
