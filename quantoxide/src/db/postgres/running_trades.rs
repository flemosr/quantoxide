use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use sqlx::{Pool, Postgres};

use lnm_sdk::api::rest::models::BoundedPercentage;
use uuid::Uuid;

use super::super::{
    error::{DbError, Result},
    models::RunningTrade,
    repositories::RunningTradesRepository,
};

pub struct PgRunningTradesRepo {
    pool: Arc<Pool<Postgres>>,
}

impl PgRunningTradesRepo {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &Pool<Postgres> {
        self.pool.as_ref()
    }
}

#[async_trait]
impl RunningTradesRepository for PgRunningTradesRepo {
    async fn register_trade(
        &self,
        trade_uuid: Uuid,
        trailing_stoploss: Option<BoundedPercentage>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
                INSERT INTO running_trades (trade_id, trailing_stoploss)
                VALUES ($1, $2)
            "#,
            trade_uuid,
            trailing_stoploss.map(|tsl| tsl.into_f64())
        )
        .execute(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }

    async fn get_trades(&self) -> Result<HashMap<Uuid, Option<BoundedPercentage>>> {
        let trades = sqlx::query_as!(
            RunningTrade,
            r#"
                SELECT trade_id, trailing_stoploss, created_at
                FROM running_trades
            "#
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        let mut result = HashMap::new();
        for trade in trades {
            let trailing_stoploss = trade
                .trailing_stoploss
                .map(BoundedPercentage::try_from)
                .transpose()
                .map_err(|e| DbError::Generic(e.to_string()))?;

            result.insert(trade.trade_id, trailing_stoploss);
        }

        Ok(result)
    }

    async fn remove_trades(&self, trade_uuids: &[Uuid]) -> Result<()> {
        if trade_uuids.is_empty() {
            return Ok(());
        }

        sqlx::query!(
            "DELETE FROM running_trades WHERE trade_id = ANY($1)",
            trade_uuids
        )
        .execute(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }
}
