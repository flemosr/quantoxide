use std::fmt;

use chrono::{DateTime, Utc};
use serde::Deserialize;
use uuid::Uuid;

use crate::shared::models::price::Price;

/// Information about a given funding fee that was paid or received, corresponding to a cross
/// margin position.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::{CrossFunding, Page};
///
/// let funding_fees: Page<CrossFunding> = rest
///     .futures_cross
///     .get_funding_fees(None, None, None, None)
///     .await?;
///
/// for fee in funding_fees.data() {
///     println!("Time: {}", fee.time());
///     println!("Settlement ID: {}", fee.settlement_id());
///     println!("Fee: {} sats", fee.fee());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CrossFunding {
    time: DateTime<Utc>,
    settlement_id: Uuid,
    fee: i64,
}

impl CrossFunding {
    /// Timestamp when the funding fee was received.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Unique identifier for the funding settlement.
    pub fn settlement_id(&self) -> Uuid {
        self.settlement_id
    }

    /// Funding fee amount in satoshis.
    pub fn fee(&self) -> i64 {
        self.fee
    }

    pub fn as_data_str(&self) -> String {
        format!(
            "time: {}\nsettlement_id: {}\nfee: {}",
            self.time.to_rfc3339(),
            self.settlement_id,
            self.fee
        )
    }
}

impl fmt::Display for CrossFunding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Cross Funding:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}

/// Information about a given funding fee that was paid or received, corresponding to an isolated
/// margin position.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::{IsolatedFunding, Page};
///
/// let funding_fees: Page<IsolatedFunding> = rest
///     .futures_isolated
///     .get_funding_fees(None, None, None, None)
///     .await?;
///
/// for fee in funding_fees.data() {
///     println!("Time: {}", fee.time());
///     println!("Settlement ID: {}", fee.settlement_id());
///     println!("Trade ID: {}", fee.trade_id());
///     println!("Fee: {} sats", fee.fee());
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct IsolatedFunding {
    time: DateTime<Utc>,
    settlement_id: Uuid,
    trade_id: Uuid,
    fee: i64,
}

impl IsolatedFunding {
    /// Timestamp when the funding fee was received.
    pub fn time(&self) -> DateTime<Utc> {
        self.time
    }

    /// Unique identifier for the funding settlement.
    pub fn settlement_id(&self) -> Uuid {
        self.settlement_id
    }

    /// Unique identifier for the trade associated with this funding.
    pub fn trade_id(&self) -> Uuid {
        self.trade_id
    }

    /// Funding fee amount in satoshis.
    pub fn fee(&self) -> i64 {
        self.fee
    }

    pub fn as_data_str(&self) -> String {
        format!(
            "time: {}\nsettlement_id: {}\ntrade_id: {}\nfee: {}",
            self.time.to_rfc3339(),
            self.settlement_id,
            self.trade_id,
            self.fee
        )
    }
}

impl fmt::Display for IsolatedFunding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Isolated Funding:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}

/// Information about a given funding settlement.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::{FundingSettlement, Page};
///
/// let settlements: Page<FundingSettlement> = rest
///     .futures_data
///     .get_funding_settlements(None, None, None, None)
///     .await?;
///
/// for settlement in settlements.data() {
///     println!("ID: {}", settlement.id());
///     println!("Time: {}", settlement.time());
///     println!("Fixing price: {}", settlement.fixing_price());
///     println!("Funding rate: {}", settlement.funding_rate());
/// }
/// # Ok(())
/// # }
/// ```
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

    pub fn as_data_str(&self) -> String {
        format!(
            "id: {}\ntime: {}\nfixing_price: {}\nfunding_rate: {:.6}",
            self.id,
            self.time.to_rfc3339(),
            self.fixing_price,
            self.funding_rate
        )
    }
}

impl fmt::Display for FundingSettlement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Funding Settlement:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
