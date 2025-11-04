use std::{collections::HashMap, sync::Arc};

use async_trait::async_trait;
use sqlx::{Pool, Postgres};
use uuid::Uuid;

use lnm_sdk::models::BoundedPercentage;

use crate::trade::core::TradeTrailingStoploss;

use super::super::{
    error::{DbError, Result},
    models::RunningTrade,
    repositories::RunningTradesRepository,
};

pub(crate) struct PgRunningTradesRepo {
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
    async fn add_running_trade(
        &self,
        trade_id: uuid::Uuid,
        trailing_stoploss: Option<TradeTrailingStoploss>,
    ) -> Result<()> {
        sqlx::query!(
            r#"
                INSERT INTO running_trades (trade_id, trailing_stoploss)
                VALUES ($1, $2)
            "#,
            trade_id,
            trailing_stoploss.map(|tsl| tsl.into_f64()),
        )
        .execute(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }

    async fn get_running_trades_map(&self) -> Result<HashMap<Uuid, Option<TradeTrailingStoploss>>> {
        let running_trades = sqlx::query_as!(
            RunningTrade,
            r#"
                SELECT trade_id, trailing_stoploss, created_at
                FROM running_trades
                ORDER BY created_at ASC
            "#
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        let mut running_trades_map = HashMap::new();

        for trade in running_trades.into_iter() {
            let trailind_stoploss = trade
                .trailing_stoploss
                .map(|tsl| {
                    BoundedPercentage::try_from(tsl)
                        .map_err(|e| {
                            DbError::UnexpectedQueryResult(format!(
                                "`trailing_stoploss` ({tsl}) cannot be casted as `BoundedPercentage`: {e}"
                            ))
                        })
                        .map(|tsl| TradeTrailingStoploss::prev_validated(tsl))
                })
                .transpose()?;

            running_trades_map.insert(trade.trade_id, trailind_stoploss);
        }

        Ok(running_trades_map)
    }

    async fn remove_running_trades(&self, trade_ids: &[Uuid]) -> Result<()> {
        sqlx::query!(
            "DELETE FROM running_trades WHERE trade_id = ANY($1)",
            trade_ids
        )
        .execute(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }
}
