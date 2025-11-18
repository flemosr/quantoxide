use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct FundingSettlement {
    settlement_id: Uuid,
    fee: i64,
    time: DateTime<Utc>,
}

impl FundingSettlement {
    /// Unique identifier for the funding settlement.
    pub fn settlement_id(&self) -> Uuid {
        self.settlement_id
    }

    /// Funding fee amount.
    pub fn fee(&self) -> i64 {
        self.fee
    }

    /// Timestamp when the funding settlement occurred.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedFundingSettlements {
    data: Vec<FundingSettlement>,
    next_cursor: Option<DateTime<Utc>>,
}

impl PaginatedFundingSettlements {
    /// Vector of funding settlements.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(settlements: lnm_sdk::api_v3::models::PaginatedFundingSettlements) -> Result<(), Box<dyn std::error::Error>> {
    /// for settlement in settlements.data() {
    ///     println!("settlement: {:?}", settlement);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn data(&self) -> &Vec<FundingSettlement> {
        &self.data
    }

    /// Cursor that can be used to fetch the next page of results. `None` if there are no more
    /// results.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(settlements: lnm_sdk::api_v3::models::PaginatedFundingSettlements) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(cursor) = settlements.next_cursor() {
    ///     println!("More settlements can be fetched using cursor: {cursor}");
    /// } else {
    ///     println!("There are no more settlements available.");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn next_cursor(&self) -> Option<DateTime<Utc>> {
        self.next_cursor
    }
}
