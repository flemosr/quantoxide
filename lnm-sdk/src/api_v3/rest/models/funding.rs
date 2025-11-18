use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

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
pub struct CrossFundingPage {
    data: Vec<CrossFunding>,
    next_cursor: Option<DateTime<Utc>>,
}

impl CrossFundingPage {
    /// Vector of cross fundings.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(cross_fundings: lnm_sdk::api_v3::models::CrossFundingPage) -> Result<(), Box<dyn std::error::Error>> {
    /// for cross_funding in cross_fundings.data() {
    ///     println!("cross_funding: {:?}", cross_funding);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn data(&self) -> &Vec<CrossFunding> {
        &self.data
    }

    /// Cursor that can be used to fetch the next page of results. `None` if there are no more
    /// results.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(cross_fundings: lnm_sdk::api_v3::models::CrossFundingPage) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(cursor) = cross_fundings.next_cursor() {
    ///     println!("More cross fundings can be fetched using cursor: {cursor}");
    /// } else {
    ///     println!("There are no more cross fundings available.");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn next_cursor(&self) -> Option<DateTime<Utc>> {
        self.next_cursor
    }
}
