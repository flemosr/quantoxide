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

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PaginatedCrossTransfers {
    data: Vec<CrossTransfer>,
    next_cursor: Option<DateTime<Utc>>,
}

impl PaginatedCrossTransfers {
    /// Vector of cross transfers.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(paginated_transfers: lnm_sdk::api_v3::models::PaginatedCrossTransfers) -> Result<(), Box<dyn std::error::Error>> {
    /// for transfer in paginated_transfers.data() {
    ///     println!("transfer: {:?}", transfer);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn data(&self) -> &Vec<CrossTransfer> {
        &self.data
    }

    /// Cursor that can be used to fetch the next page of results. `None` if there are no more
    /// results.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(paginated_transfers: lnm_sdk::api_v3::models::PaginatedCrossTransfers) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(cursor) = paginated_transfers.next_cursor() {
    ///     println!("More transfers can be fetched using cursor: {cursor}");
    /// } else {
    ///     println!("There are no more transfers available.");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn next_cursor(&self) -> Option<DateTime<Utc>> {
        self.next_cursor
    }
}
