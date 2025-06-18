use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use lnm_sdk::api::rest::models::BoundedPercentage;

use crate::trade::core::TradeTrailingStoploss;

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
        trade_tsl: Option<TradeTrailingStoploss>,
    ) -> Result<()> {
        let trailing_stoploss = trade_tsl.map(|tsl| BoundedPercentage::from(tsl).into_f64());
        sqlx::query!(
            r#"
                INSERT INTO running_trades (trade_id, trailing_stoploss)
                VALUES ($1, $2)
            "#,
            trade_uuid,
            trailing_stoploss
        )
        .execute(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }

    async fn load_and_validate_trades(
        &self,
        tsl_step_size: BoundedPercentage,
    ) -> Result<HashMap<Uuid, Option<TradeTrailingStoploss>>> {
        let registered_trades = sqlx::query_as!(
            RunningTrade,
            r#"
                SELECT trade_id, trailing_stoploss, created_at
                FROM running_trades
            "#
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        let mut valid_trade_map = HashMap::new();
        let mut invalid_trade_ids = Vec::new();

        for trade in registered_trades {
            if let Ok(trade_sl_opt) = trade
                .trailing_stoploss
                .map(BoundedPercentage::try_from)
                .transpose()
            {
                if let Ok(trade_tsl_opt) = trade_sl_opt
                    .map(|sl| TradeTrailingStoploss::new(tsl_step_size, sl))
                    .transpose()
                {
                    valid_trade_map.insert(trade.trade_id, trade_tsl_opt);
                    continue;
                }
            }

            invalid_trade_ids.push(trade.trade_id);
        }

        self.remove_trades(&invalid_trade_ids).await?;

        Ok(valid_trade_map)
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
