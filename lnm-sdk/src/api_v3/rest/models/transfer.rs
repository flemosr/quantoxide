use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CrossTransfer {
    id: Uuid,
    amount: i64,
    time: DateTime<Utc>,
}

impl CrossTransfer {
    /// Unique identifier for the cross transfer.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(transfer: lnm_sdk::api_v3::models::CrossTransfer) -> Result<(), Box<dyn std::error::Error>> {
    /// println!("Transfer ID: {}", transfer.id());
    /// # Ok(())
    /// # }
    /// ```
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Amount of the cross transfer.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(transfer: lnm_sdk::api_v3::models::CrossTransfer) -> Result<(), Box<dyn std::error::Error>> {
    /// println!("Transfer amount: {}", transfer.amount());
    /// # Ok(())
    /// # }
    /// ```
    pub fn amount(&self) -> i64 {
        self.amount
    }

    /// Timestamp when the cross transfer occurred.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(transfer: lnm_sdk::api_v3::models::CrossTransfer) -> Result<(), Box<dyn std::error::Error>> {
    /// println!("Transfer time: {}", transfer.time());
    /// # Ok(())
    /// # }
    /// ```
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }
}
