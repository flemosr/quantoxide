use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{Pool, Postgres};

use lnm_sdk::api_v3::models::FundingSettlement;

use crate::{
    sync::{
        LNM_SETTLEMENT_A_END, LNM_SETTLEMENT_B_END, LNM_SETTLEMENT_B_START, LNM_SETTLEMENT_C_START,
    },
    util::DateTimeExt,
};

use super::super::{
    error::{DbError, Result},
    models::FundingSettlementRow,
    repositories::FundingSettlementsRepository,
};

pub(crate) struct PgFundingSettlementsRepo {
    pool: Arc<Pool<Postgres>>,
}

impl PgFundingSettlementsRepo {
    pub fn new(pool: Arc<Pool<Postgres>>) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &Pool<Postgres> {
        self.pool.as_ref()
    }

    /// Finds missing settlement times using interval B (8 hours) for Phase B/C ranges.
    async fn query_missing_settlement_times_8h(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DateTime<Utc>>> {
        let missing = sqlx::query!(
            r#"
                SELECT gs.time AS "time!"
                FROM generate_series($1::timestamptz, $2::timestamptz, INTERVAL '8 hours') AS gs(time)
                LEFT JOIN funding_settlements fs ON fs.time = gs.time
                WHERE fs.time IS NULL
                ORDER BY gs.time ASC
            "#,
            from,
            to
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?
        .into_iter()
        .map(|row| row.time)
        .collect();

        Ok(missing)
    }

    /// Finds missing settlement times using interval A (24 hours) for Phase A ranges.
    async fn query_missing_settlement_times_24h(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DateTime<Utc>>> {
        let missing = sqlx::query!(
            r#"
                SELECT gs.time AS "time!"
                FROM generate_series($1::timestamptz, $2::timestamptz, INTERVAL '24 hours') AS gs(time)
                LEFT JOIN funding_settlements fs ON fs.time = gs.time
                WHERE fs.time IS NULL
                ORDER BY gs.time ASC
            "#,
            from,
            to
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?
        .into_iter()
        .map(|row| row.time)
        .collect();

        Ok(missing)
    }
}

#[async_trait]
impl FundingSettlementsRepository for PgFundingSettlementsRepo {
    async fn add_settlements(&self, settlements: &[FundingSettlement]) -> Result<()> {
        if settlements.is_empty() {
            return Ok(());
        }

        let mut ids = Vec::with_capacity(settlements.len());
        let mut times = Vec::with_capacity(settlements.len());
        let mut fixing_prices = Vec::with_capacity(settlements.len());
        let mut funding_rates = Vec::with_capacity(settlements.len());

        for settlement in settlements {
            ids.push(settlement.id());
            times.push(settlement.time());
            fixing_prices.push(settlement.fixing_price());
            funding_rates.push(settlement.funding_rate());
        }

        sqlx::query!(
            r#"
                INSERT INTO funding_settlements (id, time, fixing_price, funding_rate)
                SELECT * FROM unnest($1::uuid[], $2::timestamptz[], $3::float8[], $4::float8[])
                ON CONFLICT (id) DO NOTHING
            "#,
            &ids,
            &times,
            &fixing_prices,
            &funding_rates,
        )
        .execute(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(())
    }

    async fn get_settlements(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<FundingSettlementRow>> {
        let rows = sqlx::query_as!(
            FundingSettlementRow,
            r#"
                SELECT id, time, fixing_price, funding_rate, created_at
                FROM funding_settlements
                WHERE time >= $1 AND time <= $2
                ORDER BY time ASC
            "#,
            from,
            to
        )
        .fetch_all(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(rows)
    }

    async fn get_earliest_settlement_time(&self) -> Result<Option<DateTime<Utc>>> {
        struct TimeRow {
            pub time: DateTime<Utc>,
        }

        let row = sqlx::query_as!(
            TimeRow,
            r#"
                SELECT time
                FROM funding_settlements
                ORDER BY time ASC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(row.map(|r| r.time))
    }

    async fn get_latest_settlement_time(&self) -> Result<Option<DateTime<Utc>>> {
        struct TimeRow {
            pub time: DateTime<Utc>,
        }

        let row = sqlx::query_as!(
            TimeRow,
            r#"
                SELECT time
                FROM funding_settlements
                ORDER BY time DESC
                LIMIT 1
            "#
        )
        .fetch_optional(self.pool())
        .await
        .map_err(DbError::Query)?;

        Ok(row.map(|r| r.time))
    }

    async fn get_missing_settlement_times(
        &self,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<DateTime<Utc>>> {
        if !from.is_valid_funding_settlement_time() {
            return Err(DbError::InvalidFundingSettlementTime { time: from });
        }
        if !to.is_valid_funding_settlement_time() {
            return Err(DbError::InvalidFundingSettlementTime { time: to });
        }

        // Split cross-phase ranges so each `generate_series` query uses the correct interval
        // and starts on a grid-aligned anchor for that phase.
        let mut combined = Vec::new();

        // Phase A segment (interval A, 24h): from .. min(to, PHASE_A_END)
        if from <= LNM_SETTLEMENT_A_END {
            let phase_a_to = to.min(LNM_SETTLEMENT_A_END);
            combined.extend(
                self.query_missing_settlement_times_24h(from, phase_a_to)
                    .await?,
            );
        }

        // Phase B segment (interval B, 8h): max(from, PHASE_B_START) .. min(to, PHASE_B_END)
        if from <= LNM_SETTLEMENT_B_END && to >= LNM_SETTLEMENT_B_START {
            let phase_b_from = from.max(LNM_SETTLEMENT_B_START);
            let phase_b_to = to.min(LNM_SETTLEMENT_B_END);
            combined.extend(
                self.query_missing_settlement_times_8h(phase_b_from, phase_b_to)
                    .await?,
            );
        }

        // Phase C segment (interval B, 8h): max(from, PHASE_C_START) .. to
        if to >= LNM_SETTLEMENT_C_START {
            let phase_c_from = from.max(LNM_SETTLEMENT_C_START);
            combined.extend(
                self.query_missing_settlement_times_8h(phase_c_from, to)
                    .await?,
            );
        }

        Ok(combined)
    }
}
