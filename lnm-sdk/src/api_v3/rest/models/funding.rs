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
pub struct IsolatedFundingPage {
    data: Vec<IsolatedFunding>,
    next_cursor: Option<DateTime<Utc>>,
}

impl IsolatedFundingPage {
    /// Vector of isolated fundings.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(isolated_fundings: lnm_sdk::api_v3::models::IsolatedFundingPage) -> Result<(), Box<dyn std::error::Error>> {
    /// for isolated_funding in isolated_fundings.data() {
    ///     println!("isolated_funding: {:?}", isolated_funding);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn data(&self) -> &Vec<IsolatedFunding> {
        &self.data
    }

    /// Cursor that can be used to fetch the next page of results. `None` if there are no more
    /// results.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(isolated_fundings: lnm_sdk::api_v3::models::IsolatedFundingPage) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(cursor) = isolated_fundings.next_cursor() {
    ///     println!("More isolated fundings can be fetched using cursor: {cursor}");
    /// } else {
    ///     println!("There are no more isolated fundings available.");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn next_cursor(&self) -> Option<DateTime<Utc>> {
        self.next_cursor
    }
}
