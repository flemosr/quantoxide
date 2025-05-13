use async_trait::async_trait;

use lnm_sdk::api::rest::models::{BoundedPercentage, Leverage, LowerBoundedPercentage};

use super::{TradesManager, TradesState, error::Result};

pub struct LiveTradesManager {
    max_running_qtd: usize,
}

impl LiveTradesManager {
    pub fn new(max_running_qtd: usize) -> Self {
        Self { max_running_qtd }
    }
}

#[async_trait]
impl TradesManager for LiveTradesManager {
    async fn open_long(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: LowerBoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        todo!()
    }

    async fn open_short(
        &self,
        stoploss_perc: BoundedPercentage,
        takeprofit_perc: BoundedPercentage,
        balance_perc: BoundedPercentage,
        leverage: Leverage,
    ) -> Result<()> {
        todo!()
    }

    async fn close_longs(&self) -> Result<()> {
        todo!()
    }

    async fn close_shorts(&self) -> Result<()> {
        todo!()
    }

    async fn close_all(&self) -> Result<()> {
        todo!()
    }

    async fn state(&self) -> Result<TradesState> {
        todo!()
    }
}
