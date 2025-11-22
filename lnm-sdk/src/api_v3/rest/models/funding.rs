use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::shared::models::price::Price;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CrossFunding {
    settlement_id: Uuid,
    fee: i64,
    time: DateTime<Utc>,
}

impl CrossFunding {
    /// Unique identifier for the funding settlement.
    pub fn settlement_id(&self) -> Uuid {
        self.settlement_id
    }

    /// Funding fee amount.
    pub fn fee(&self) -> i64 {
        self.fee
    }

    /// Timestamp when the funding fee was received.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IsolatedFunding {
    settlement_id: Uuid,
    trade_id: Uuid,
    fee: i64,
    time: DateTime<Utc>,
}

impl IsolatedFunding {
    /// Unique identifier for the funding settlement.
    pub fn settlement_id(&self) -> Uuid {
        self.settlement_id
    }

    /// Unique identifier for the trade associated with this funding.
    pub fn trade_id(&self) -> Uuid {
        self.trade_id
    }

    /// Funding fee amount.
    pub fn fee(&self) -> i64 {
        self.fee
    }

    /// Timestamp when the funding fee was received.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FundingSettlement {
    id: Uuid,
    time: DateTime<Utc>,
    fixing_price: Price,
    funding_rate: f64,
}

impl FundingSettlement {
    /// Unique identifier for the funding settlement.
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Timestamp of the funding settlement.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// The fixing price used for the funding settlement.
    pub fn fixing_price(&self) -> Price {
        self.fixing_price
    }

    /// The funding rate applied.
    pub fn funding_rate(&self) -> f64 {
        self.funding_rate
    }
}
