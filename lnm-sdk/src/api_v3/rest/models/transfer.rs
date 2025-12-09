use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

/// A transfer between the isolated account and the cross-margin account.
///
/// Represents a transfer of funds that moves collateral between the user's isolated futures
/// account and their cross-margin account. Positive amounts indicate transfers to the cross
/// account, while negative amounts indicate transfers from the cross account to the isolated
/// account.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest_api: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::{Page, CrossTransfer};
///
/// // Get transfer history for cross margin account
/// let transfers: Page<CrossTransfer> = rest_api
///     .futures_cross
///     .get_transfers(None, None, None, None)
///     .await?;
///
/// for transfer in transfers.data() {
///     println!("Transfer ID: {}", transfer.id());
///     if transfer.amount() > 0 {
///         println!("Deposit: {} sats", transfer.amount());
///     } else {
///         println!("Withdrawal: {} sats", transfer.amount().abs());
///     }
///     println!("Time: {}", transfer.time());
/// }
/// # Ok(())
/// # }
/// ```
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
